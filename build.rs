use std::fs;

fn main() {
    return;

    let _ = fs::create_dir("idls");
    if !fs::exists("./idls/memos.json").unwrap() {
        // Todo: Automatically invoke a memos build?

        fs::copy("./memos/target/idl/memos.json", "idls/memos.json")
            .expect("Missing memos idl. Please build the memos anchor project first.");
    }
}
