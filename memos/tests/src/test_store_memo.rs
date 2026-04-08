use anchor_client::{Client, Cluster, CommitmentConfig, Signer};
use solana_keypair::{Keypair, read_keypair_file};
use solana_pubkey::Pubkey;

#[test]
fn test_store_memo() {
    let program_id = "H7vhVbT3VHTTBNN6Um6WQT956TD2LEgQUgbJxaeuKtFu";
    let anchor_wallet = std::env::var("ANCHOR_WALLET").unwrap();
    let payer = read_keypair_file(&anchor_wallet).unwrap();

    let client = Client::new_with_options(Cluster::Localnet, &payer, CommitmentConfig::confirmed());
    let program_id = Pubkey::try_from(program_id).unwrap();
    let program = client.program(program_id).unwrap();

    let memo_keypair = Keypair::new();
    let text = "hello there".to_string();

    program
        .request()
        .accounts(memos::accounts::StoreMemo {
            memo: memo_keypair.pubkey(),
            signer: payer.pubkey(),
            system_program: Pubkey::default(),
        })
        .args(memos::instruction::StoreMemo { text: text.clone() })
        .signer(&memo_keypair)
        .send()
        .expect("Transaction failed");

    let memo_account: memos::Memo = program.account(memo_keypair.pubkey()).unwrap();
    assert_eq!(memo_account.memo, text);
    assert_ne!(memo_account.timestamp, 0);

    // Now we try to create a memo that is too large.
    let memo_keypair = Keypair::new();
    let text = String::from_utf8(vec![b'!'; memos::MAX_MEMO_SIZE + 1]).unwrap();

    // Transaction creating the memo should not succeed.
    let result = program
        .request()
        .accounts(memos::accounts::StoreMemo {
            memo: memo_keypair.pubkey(),
            signer: payer.pubkey(),
            system_program: Pubkey::default(),
        })
        .args(memos::instruction::StoreMemo { text: text.clone() })
        .signer(&memo_keypair)
        .send();

    let Err(e) = result else {
        panic!("Expected an error.");
    };
    assert!(e.to_string().contains("A raw constraint was violated."));

    // Account holding the memo should not exist
    let result = program.account::<memos::Memo>(memo_keypair.pubkey());
    assert!(result.is_err());
}
