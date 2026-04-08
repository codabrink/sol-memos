use anchor_client::{Client, Cluster, CommitmentConfig, Signer};
use solana_keypair::read_keypair_file;
use solana_pubkey::Pubkey;

#[test]
fn test_store_memo() {
    let program_id = Pubkey::try_from("6e3aABH3MD1DzDy86CdJefjt1qa2QsJxwDxMHu6iNigy").unwrap();
    let anchor_wallet = std::env::var("ANCHOR_WALLET").unwrap();
    let payer = read_keypair_file(&anchor_wallet).unwrap();

    let client = Client::new_with_options(Cluster::Localnet, &payer, CommitmentConfig::confirmed());
    let program = client.program(program_id).unwrap();

    let (counter_pda, _) =
        Pubkey::find_program_address(&[payer.pubkey().as_ref(), b"counter"], &program_id);

    // Get the current count (0 if counter doesn't exist yet).
    let count: u64 = program
        .account::<memos::MemoCounter>(counter_pda)
        .map(|c| c.count)
        .unwrap_or(0);

    let (memo_pda, _) = Pubkey::find_program_address(
        &[payer.pubkey().as_ref(), b"memo", &count.to_le_bytes()],
        &program_id,
    );

    let text = "hello there".to_string();

    program
        .request()
        .accounts(memos::accounts::StoreMemo {
            counter: counter_pda,
            memo: memo_pda,
            signer: payer.pubkey(),
            system_program: Pubkey::default(),
        })
        .args(memos::instruction::StoreMemo { text: text.clone() })
        .send()
        .expect("Transaction failed");

    let memo_account: memos::Memo = program.account(memo_pda).unwrap();
    assert_eq!(memo_account.memo, text);
    assert_eq!(memo_account.count, count);

    // Now we try to create a memo that is too large.
    let (memo_pda_large, _) = Pubkey::find_program_address(
        &[
            payer.pubkey().as_ref(),
            b"memo",
            &(count + 1).to_le_bytes(),
        ],
        &program_id,
    );
    let text = String::from_utf8(vec![b'!'; memos::MAX_MEMO_SIZE + 1]).unwrap();

    // Transaction creating the memo should not succeed.
    let result = program
        .request()
        .accounts(memos::accounts::StoreMemo {
            counter: counter_pda,
            memo: memo_pda_large,
            signer: payer.pubkey(),
            system_program: Pubkey::default(),
        })
        .args(memos::instruction::StoreMemo { text: text.clone() })
        .send();

    let Err(e) = result else {
        panic!("Expected an error.");
    };
    assert!(e.to_string().contains("A raw constraint was violated."));

    // Account holding the memo should not exist.
    let result = program.account::<memos::Memo>(memo_pda_large);
    assert!(result.is_err());
}
