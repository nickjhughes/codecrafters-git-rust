use anyhow::Result;
use clap::{Parser, Subcommand};
use std::{fs, path::PathBuf};

use object::Object;

mod object;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init,
    CatFile {
        #[arg(short)]
        pretty_print: bool,
        object_name: String,
    },
    HashObject {
        #[arg(short)]
        write: bool,
        path: PathBuf,
    },
    LsTree {
        #[arg(long)]
        name_only: bool,
        tree_hash: String,
    },
    WriteTree,
    CommitTree {
        tree_hash: String,
        #[arg(short)]
        parents: Vec<String>,
        #[arg(short)]
        message: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            fs::create_dir(".git")?;
            fs::create_dir(".git/objects")?;
            fs::create_dir(".git/refs")?;
            fs::write(".git/HEAD", "ref: refs/heads/master\n")?;
            println!("Initialized git directory")
        }
        Commands::CatFile {
            pretty_print,
            object_name,
        } => {
            assert!(pretty_print);

            // Should be a SHA1 hash
            assert_eq!(object_name.len(), 40);

            let mut path = PathBuf::from(".git/objects");
            path.push(&object_name[0..2]);
            path.push(&object_name[2..40]);

            let object = Object::parse(path)?;
            assert!(matches!(object, Object::Blob(_)));
            object.print();
        }
        Commands::HashObject { write, path } => {
            assert!(write);

            let object = Object::new_from_path(path)?;
            object.add()?;
            println!("{}", object.hash());
        }
        Commands::LsTree {
            name_only,
            tree_hash,
        } => {
            assert!(name_only);

            // Should be a SHA1 hash
            assert_eq!(tree_hash.len(), 40);

            let mut path = PathBuf::from(".git/objects");
            path.push(&tree_hash[0..2]);
            path.push(&tree_hash[2..40]);

            let object = Object::parse(path)?;
            assert!(matches!(object, Object::Tree(_)));
            object.print();
        }
        Commands::WriteTree => {
            let object = Object::new_from_path(PathBuf::from("./"))?;
            object.add()?;
            println!("{}", object.hash());
        }
        Commands::CommitTree {
            tree_hash,
            parents,
            message,
        } => {
            // Should be a SHA1 hash
            assert_eq!(tree_hash.len(), 40);

            assert_eq!(parents.len(), 1);
            let parent_hash = parents.first().unwrap();
            // Should be a SHA1 hash
            assert_eq!(parent_hash.len(), 40);

            let object = Object::new_commit(&tree_hash, Some(parent_hash), &message);
            object.add()?;
            println!("{}", object.hash());
        }
    }

    Ok(())
}
