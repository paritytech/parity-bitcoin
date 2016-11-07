use clap::ArgMatches;
use sync::{create_sync_blocks_writer, Error};
use config::Config;
use util::{open_db, init_db};

pub fn import(cfg: Config, matches: &ArgMatches) -> Result<(), String> {
	let db = open_db(&cfg);
	// TODO: this might be unnecessary here!
	init_db(&cfg, &db);

	let mut writer = create_sync_blocks_writer(db);

	let blk_path = matches.value_of("PATH").expect("PATH is required in cli.yml; qed");
	let blk_dir = try!(::import::open_blk_dir(blk_path).map_err(|_| "Import directory does not exist".to_owned()));
	let mut counter = 0;
	let mut skipped = 0;
	for blk in blk_dir {
		// TODO: verify magic!
		let blk = try!(blk.map_err(|_| "Cannot read block".to_owned()));
		match writer.append_block(blk.block) {
			Ok(_) => {
				counter += 1;
				if counter % 1000 == 0 {
					info!("Imported {} blocks", counter);
				}
			}
			Err(Error::OutOfOrderBlock) => {
				skipped += 1;
			},
			Err(_) => return Err(format!("Cannot append block")),
		}
	}

	info!("Finished import of {} blocks. Skipped {} blocks.", counter, skipped);

	Ok(())
}
