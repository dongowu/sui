// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::execution_scheduler::balance_withdraw_scheduler::ScheduleResult;
use crate::execution_scheduler::balance_withdraw_scheduler::{
    balance_read::MockBalanceRead, scheduler::BalanceWithdrawScheduler, BalanceSettlement,
    ScheduleStatus, TxBalanceWithdraw,
};
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use rand::{seq::SliceRandom, Rng};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    digests::TransactionDigest,
};
use tokio::sync::oneshot;
use tokio::time::timeout;

#[derive(Clone)]
struct TestScheduler {
    mock_read: Arc<MockBalanceRead>,
    scheduler: Arc<BalanceWithdrawScheduler>,
}

impl TestScheduler {
    fn new(init_version: SequenceNumber, init_balances: BTreeMap<ObjectID, u128>) -> Self {
        let mock_read = Arc::new(MockBalanceRead::new(init_version, init_balances));
        let scheduler = BalanceWithdrawScheduler::new(mock_read.clone(), init_version);
        Self {
            mock_read,
            scheduler,
        }
    }

    fn settle_balance_changes(&self, changes: BTreeMap<ObjectID, i128>) {
        self.mock_read.settle_balance_changes(changes.clone());
        self.scheduler.settle_balances(BalanceSettlement {
            balance_changes: changes,
        });
    }
}

async fn wait_for_results(
    mut receivers: FuturesUnordered<oneshot::Receiver<ScheduleResult>>,
    expected_results: BTreeMap<TransactionDigest, ScheduleStatus>,
) {
    timeout(Duration::from_secs(3), async {
        let mut results = BTreeMap::new();
        while let Some(result) = receivers.next().await {
            let result = result.unwrap();
            results.insert(result.tx_digest, result.status);
        }
        assert_eq!(results, expected_results);
    })
    .await
    .unwrap();
}

#[tokio::test]
#[should_panic(expected = "Elapsed")]
async fn test_schedule_wait_for_settlement() {
    // This test checks that a withdraw cannot be scheduled until
    // a settlement, and if there is no settlement we would lose liveness.
    let init_version = SequenceNumber::from_u64(0);
    let account = ObjectID::random();
    let test = TestScheduler::new(init_version, BTreeMap::from([(account, 100)]));

    let withdraw = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(account, 200)]),
    };

    let receivers = test
        .scheduler
        .schedule_withdraws(init_version.next(), vec![withdraw.clone()]);
    wait_for_results(
        receivers,
        BTreeMap::from([(withdraw.tx_digest, ScheduleStatus::SufficientBalance)]),
    )
    .await;
}

#[tokio::test]
async fn test_schedules_and_settles() {
    let v0 = SequenceNumber::from_u64(0);
    let account = ObjectID::random();
    let test = TestScheduler::new(v0, BTreeMap::from([(account, 100)]));

    let withdraw0 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(account, 60)]),
    };
    let withdraw1 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(account, 60)]),
    };
    let withdraw2 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(account, 60)]),
    };

    let receivers = test
        .scheduler
        .schedule_withdraws(v0, vec![withdraw0.clone()]);

    wait_for_results(
        receivers,
        BTreeMap::from([(withdraw0.tx_digest, ScheduleStatus::SufficientBalance)]),
    )
    .await;

    let v1 = v0.next();
    let receivers = test
        .scheduler
        .schedule_withdraws(v1, vec![withdraw1.clone()]);

    // 100 -> 40, v0 -> v1
    test.settle_balance_changes(BTreeMap::from([(account, -60)]));

    wait_for_results(
        receivers,
        BTreeMap::from([(withdraw1.tx_digest, ScheduleStatus::InsufficientBalance)]),
    )
    .await;

    let v2 = v1.next();
    let receivers = test
        .scheduler
        .schedule_withdraws(v2, vec![withdraw2.clone()]);

    // 40 -> 60, v1 -> v2
    test.settle_balance_changes(BTreeMap::from([(account, 20)]));

    wait_for_results(
        receivers,
        BTreeMap::from([(withdraw2.tx_digest, ScheduleStatus::SufficientBalance)]),
    )
    .await;
}

#[tokio::test]
async fn test_already_executed() {
    let init_version = SequenceNumber::from_u64(0);
    let account1 = ObjectID::random();
    let account2 = ObjectID::random();
    let test = TestScheduler::new(
        init_version,
        BTreeMap::from([(account1, 100), (account2, 200)]),
    );

    // Advance the accumulator version
    test.settle_balance_changes(BTreeMap::new());

    tokio::time::sleep(Duration::from_millis(10)).await;

    // Try to schedule multiple withdraws for the old version
    let withdraw1 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(account1, 50)]),
    };
    let withdraw2 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(account2, 100)]),
    };

    let receivers = test
        .scheduler
        .schedule_withdraws(init_version, vec![withdraw1.clone(), withdraw2.clone()]);
    wait_for_results(
        receivers,
        BTreeMap::from([
            (withdraw1.tx_digest, ScheduleStatus::AlreadyExecuted),
            (withdraw2.tx_digest, ScheduleStatus::AlreadyExecuted),
        ]),
    )
    .await;
}

#[tokio::test]
async fn test_multiple_withdraws_same_version() {
    // This test checks that even though the second withdraw failed due to insufficient balance,
    // the third withdraw can still be scheduled since the second withdraw does not reserve any balance.
    let init_version = SequenceNumber::from_u64(0);
    let account = ObjectID::random();
    let test = TestScheduler::new(init_version, BTreeMap::from([(account, 90)]));

    let withdraw1 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(account, 50)]),
    };
    let withdraw2 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(account, 50)]),
    };
    let withdraw3 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(account, 40)]),
    };

    let receivers = test.scheduler.schedule_withdraws(
        init_version,
        vec![withdraw1.clone(), withdraw2.clone(), withdraw3.clone()],
    );
    wait_for_results(
        receivers,
        BTreeMap::from([
            (withdraw1.tx_digest, ScheduleStatus::SufficientBalance),
            (withdraw2.tx_digest, ScheduleStatus::InsufficientBalance),
            (withdraw3.tx_digest, ScheduleStatus::SufficientBalance),
        ]),
    )
    .await;
}

#[tokio::test]
async fn test_multiple_withdraws_multiple_accounts_same_version() {
    let init_version = SequenceNumber::from_u64(0);
    let account1 = ObjectID::random();
    let account2 = ObjectID::random();
    let test = TestScheduler::new(
        init_version,
        BTreeMap::from([(account1, 100), (account2, 100)]),
    );

    let withdraw1 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(account1, 100), (account2, 200)]),
    };
    let withdraw2 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(account1, 1)]),
    };
    let withdraw3 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(account2, 100)]),
    };

    let receivers = test.scheduler.schedule_withdraws(
        init_version,
        vec![withdraw1.clone(), withdraw2.clone(), withdraw3.clone()],
    );
    wait_for_results(
        receivers,
        BTreeMap::from([
            (withdraw1.tx_digest, ScheduleStatus::InsufficientBalance),
            (withdraw2.tx_digest, ScheduleStatus::InsufficientBalance),
            (withdraw3.tx_digest, ScheduleStatus::SufficientBalance),
        ]),
    )
    .await;
}

#[tokio::test]
async fn balance_withdraw_scheduler_stress_test() {
    telemetry_subscribers::init_for_testing();

    let num_accounts = 5;
    let num_transactions = 10000;

    let mut version = SequenceNumber::from_u64(0);
    let accounts = (0..num_accounts)
        .map(|_| ObjectID::random())
        .collect::<Vec<_>>();
    let mut rng = rand::thread_rng();
    let init_balances = accounts
        .iter()
        .filter_map(|account_id| {
            if rng.gen_bool(0.7) {
                Some((*account_id, rng.gen_range(0..20)))
            } else {
                None
            }
        })
        .collect::<BTreeMap<_, _>>();
    tracing::debug!("Init balances: {:?}", init_balances);

    let mut withdraws = Vec::new();
    let mut settlements = Vec::new();
    let mut cur_reservations = Vec::new();

    let mut current_balances = init_balances.clone();
    for idx in 0..num_transactions {
        let num_reservation_accounts = rng.gen_range(1..3);
        let account_ids = accounts
            .choose_multiple(&mut rng, num_reservation_accounts)
            .cloned()
            .collect::<Vec<_>>();
        let reservations = account_ids
            .iter()
            .map(|account_id| (*account_id, rng.gen_range(1..10)))
            .collect::<BTreeMap<_, _>>();
        cur_reservations.push(TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations,
        });
        // Every now and then we generate a settlement to advance the version.
        // We don't really settle any balance changes here, as this test
        // is primarily focusing on the scheduler's ability to handle
        // random combinations ofwithdraws reservations.
        if rng.gen_bool(0.2) || idx == num_transactions - 1 {
            // Generate random balance changes for some of the accounts,
            // negative values mean withdraws, positive values mean deposits.
            // Only accounts that had reservations can have negative balance changes.
            // The absolute value of Withdraws must be bounded by the current balance.
            let affected_accounts: HashSet<_> = cur_reservations
                .iter()
                .flat_map(|withdraw| withdraw.reservations.keys().cloned())
                .collect();
            let num_changes = rng.gen_range(0..num_accounts);
            let balance_changes = accounts
                .choose_multiple(&mut rng, num_changes)
                .map(|account_id| {
                    let cur_balance = current_balances
                        .get(account_id)
                        .copied()
                        .unwrap_or_default() as i128;
                    let change = if affected_accounts.contains(account_id) {
                        rng.gen_range(-cur_balance..20)
                    } else {
                        rng.gen_range(0..10)
                    };
                    (*account_id, change)
                })
                .collect::<BTreeMap<_, _>>();
            for (account_id, change) in balance_changes.iter() {
                let entry = current_balances.entry(*account_id).or_insert(0);
                *entry = (*entry as i128 + change) as u128;
            }
            withdraws.push((version, std::mem::take(&mut cur_reservations)));
            settlements.push((version, balance_changes));
            version = version.next();
        }
    }

    // Run through the scheduler many times and check that the results are always the same.
    let mut expected_results = None;
    let mut handles = Vec::new();

    // Spawn 10 concurrent tasks
    for _ in 0..10 {
        let init_balances = init_balances.clone();
        let settlements = settlements.clone();
        let withdraws = withdraws.clone();

        let handle = tokio::spawn(async move {
            let test = TestScheduler::new(SequenceNumber::from_u64(0), init_balances);

            // Start a separate thread to run all settlements on the scheduler.
            let test_clone = test.clone();
            let settlements = settlements.clone();
            let last_scheduled_version = Arc::new(AtomicU64::new(0));
            let last_scheduled_version_clone = last_scheduled_version.clone();
            let _settle_task = tokio::spawn(async move {
                for (version, balance_changes) in settlements {
                    // We can only settle after the version has been scheduled.
                    // This is to avoid non-determinism in the test where some withdraws
                    // may end up with AlreadyExecuted status if we settle before the version
                    // has been scheduled.
                    while last_scheduled_version_clone.load(Ordering::Relaxed) < version.value() {
                        tokio::time::sleep(Duration::from_millis(1)).await;
                    }
                    let delay_ms = rand::thread_rng().gen_range(0..3);
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    test_clone.settle_balance_changes(balance_changes);
                }
            });

            let mut all_receivers = Vec::new();
            for (version, withdraws) in withdraws {
                let receivers = test.scheduler.schedule_withdraws(version, withdraws);
                all_receivers.push((version, receivers));
            }

            let mut results = BTreeMap::new();
            for (version, receivers) in all_receivers {
                for result in receivers {
                    let result = result.await.unwrap();
                    results.insert(result.tx_digest, result.status);
                }
                last_scheduled_version.store(version.value(), Ordering::Relaxed);
            }
            results
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete and check that all results are the same.
    for handle in handles {
        let results = handle.await.unwrap();
        if expected_results.is_none() {
            expected_results = Some(results);
        } else {
            assert_eq!(expected_results, Some(results));
        }
    }
}
