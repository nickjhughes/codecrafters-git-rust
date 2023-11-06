use anyhow::Result;
use clap::{Parser, Subcommand};
use std::{fs, path::PathBuf};

use object::Object;

mod object;
mod pack;
mod transfer;
mod util;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Add,
    CatFile {
        #[arg(short)]
        pretty_print: bool,
        object_name: String,
    },
    CheckIgnore,
    Checkout,
    Clone {
        repo_url: reqwest::Url,
        directory: PathBuf,
    },
    Commit,
    CommitTree {
        tree_hash: String,
        #[arg(short)]
        parents: Vec<String>,
        #[arg(short)]
        message: String,
    },
    HashObject {
        #[arg(short)]
        write: bool,
        path: PathBuf,
    },
    Init,
    Log,
    LsFiles,
    LsRemote {
        repo_url: reqwest::Url,
    },
    LsTree {
        #[arg(long)]
        name_only: bool,
        tree_hash: String,
    },
    RevParse,
    Rm,
    ShowRef,
    Status,
    Tag,
    WriteTree,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Add => add(),
        Commands::CatFile {
            pretty_print,
            object_name,
        } => cat_file(&object_name, pretty_print),
        Commands::CheckIgnore => check_ignore(),
        Commands::Checkout => checkout(),
        Commands::Clone {
            repo_url,
            directory,
        } => clone(repo_url, directory),
        Commands::Commit => commit(),
        Commands::CommitTree {
            tree_hash,
            parents,
            message,
        } => commit_tree(&tree_hash, &parents, &message),
        Commands::HashObject { write, path } => hash_object(path, write),
        Commands::Init => init(),
        Commands::Log => log(),
        Commands::LsFiles => ls_files(),
        Commands::LsRemote { repo_url } => ls_remote(repo_url),
        Commands::LsTree {
            name_only,
            tree_hash,
        } => ls_tree(&tree_hash, name_only),
        Commands::RevParse => rev_parse(),
        Commands::Rm => rm(),
        Commands::ShowRef => show_ref(),
        Commands::Status => status(),
        Commands::Tag => tag(),
        Commands::WriteTree => write_tree(),
    }
}

fn add() -> Result<()> {
    todo!("add")
}

fn cat_file(object_name: &str, pretty_print: bool) -> Result<()> {
    assert!(pretty_print);

    // Should be a SHA1 hash
    assert_eq!(object_name.len(), 40);

    let mut path = PathBuf::from(".git/objects");
    path.push(&object_name[0..2]);
    path.push(&object_name[2..40]);

    let object = Object::parse_from_path(path)?;
    assert!(matches!(object, Object::Blob(_)));
    object.print();

    Ok(())
}

fn check_ignore() -> Result<()> {
    todo!("check_ignore")
}

fn checkout() -> Result<()> {
    todo!("checkout")
}

fn clone(repo_url: reqwest::Url, _directory: PathBuf) -> Result<()> {
    // std::fs::create_dir_all(directory)?;
    transfer::clone(repo_url)
}

fn commit() -> Result<()> {
    todo!("commit")
}

fn commit_tree(tree_hash: &str, parents: &[String], message: &str) -> Result<()> {
    // Should be a SHA1 hash
    assert_eq!(tree_hash.len(), 40);

    assert_eq!(parents.len(), 1);
    let parent_hash = parents.first().unwrap();
    // Should be a SHA1 hash
    assert_eq!(parent_hash.len(), 40);

    let object = Object::new_commit(tree_hash, Some(parent_hash), message);
    object.add()?;
    println!("{}", object.hash());

    Ok(())
}

fn hash_object(path: PathBuf, write: bool) -> Result<()> {
    assert!(write);

    let object = Object::new_from_path(path)?;
    object.add()?;
    println!("{}", object.hash());

    Ok(())
}

fn init() -> Result<()> {
    fs::create_dir(".git")?;
    fs::create_dir(".git/objects")?;
    fs::create_dir(".git/refs")?;
    fs::write(".git/HEAD", "ref: refs/heads/master\n")?;
    println!("Initialized git directory");
    Ok(())
}

fn log() -> Result<()> {
    todo!("log")
}

fn ls_files() -> Result<()> {
    todo!("ls_files")
}

fn ls_remote(repo_url: reqwest::Url) -> Result<()> {
    let (refs, _) = transfer::get_refs(&repo_url)?;
    for ref_ in refs.iter() {
        ref_.print();
    }
    Ok(())
}

fn ls_tree(tree_hash: &str, name_only: bool) -> Result<()> {
    assert!(name_only);

    // Should be a SHA1 hash
    assert_eq!(tree_hash.len(), 40);

    let mut path = PathBuf::from(".git/objects");
    path.push(&tree_hash[0..2]);
    path.push(&tree_hash[2..40]);

    let object = Object::parse_from_path(path)?;
    assert!(matches!(object, Object::Tree(_)));
    object.print();

    Ok(())
}

fn rev_parse() -> Result<()> {
    todo!("rev_parse")
}
fn rm() -> Result<()> {
    todo!("rm")
}
fn show_ref() -> Result<()> {
    todo!("show_ref")
}
fn status() -> Result<()> {
    todo!("status")
}
fn tag() -> Result<()> {
    todo!("tag")
}

fn write_tree() -> Result<()> {
    let object = Object::new_from_path(PathBuf::from("./"))?;
    object.add()?;
    println!("{}", object.hash());
    Ok(())
}
