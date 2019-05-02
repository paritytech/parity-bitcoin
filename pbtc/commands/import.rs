use clap::ArgMatches;
use sync::{create_sync_blocks_writer, Error};
use config::Config;
use util::init_db;

pub fn import(cfg: Config, matches: &ArgMatches) -> Result<(), String> {
	try!(init_db(&cfg));

	let blk_path = matches.value_of("PATH").expect("PATH is required in cli.yml; qed");
	let blk_dir = ::import::open_blk_dir(blk_path)
		.map_err(|err| format!("Failed to open import directory: {}", err))?;

	let mut writer = create_sync_blocks_writer(cfg.db, cfg.consensus, cfg.verification_params);
	let mut counter = 0;
	let mut previous_hash = None;
	for blk in blk_dir {
		// TODO: verify magic!
		let blk = blk.map_err(|err| format!("Cannot read block: {:?}. Previous block: {:?}", err, previous_hash))?;
		let blk_hash = blk.block.hash().reversed();
		match writer.append_block(blk.block) {
			Ok(_) => {
				counter += 1;
				if counter % 1000 == 0 {
					info!(target: "sync", "Imported {} blocks", counter);
				}
			}
			Err(Error::TooManyOrphanBlocks) => return Err("Too many orphan (unordered) blocks".into()),
			Err(err) => return Err(format!("Cannot append block: {:?}. Block: {}", err, blk_hash)),
		}

		previous_hash = Some(blk_hash);
	}

	info!("Finished import of {} blocks", counter);

	Ok(())
}
