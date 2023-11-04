use anyhow::Result;
use std::{env, fs, path::PathBuf};

use object::Object;

mod object;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    match args[1].as_str() {
        "init" => {
            fs::create_dir(".git")?;
            fs::create_dir(".git/objects")?;
            fs::create_dir(".git/refs")?;
            fs::write(".git/HEAD", "ref: refs/heads/master\n")?;
            println!("Initialized git directory")
        }
        "cat-file" => {
            assert_eq!(args[2], "-p");
            let object_name = &args[3];
            // Should be a SHA1 hash
            assert_eq!(object_name.len(), 40);

            let mut path = PathBuf::from(".git/objects");
            path.push(&object_name[0..2]);
            path.push(&object_name[2..40]);

            let object = Object::parse(path)?;
            object.print();
        }
        "hash-object" => {
            assert_eq!(args[2], "-w");
            let path = PathBuf::from(&args[3]);

            let object = Object::new(path)?;
            object.add()?;
            println!("{}", object.hash());
        }
        cmd => {
            anyhow::bail!("unknown command: {cmd}")
        }
    }
    Ok(())
}
