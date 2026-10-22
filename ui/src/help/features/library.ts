import { msg } from "../../i18n";
import type { Feature } from "../features";

/** The read-only SYRF track library (asked 2026-10-04): its folders, and searching and importing what they hold. */
const reveal = ["settings:", "settings:library"];
const keywords = ["SYRF", "GeoJSON", msg("library")];
const dbKeywords = [...keywords, "PostgreSQL", msg("database")];
const features: Feature[] = [
  { id: "settings:library-geojson", label: msg("GeoJSON track directory"), description: msg("The folder of track files the boat metadata names."), keywords, topic: "settings", reveal },
  { id: "settings:library-geojson-browse", label: msg("Choose GeoJSON track directory"), description: msg("Choose GeoJSON track directory"), keywords, topic: "settings", reveal },
  { id: "settings:library-metadata", label: msg("Boat metadata directory"), description: msg("Leave empty to use the application data directory"), keywords, topic: "settings", reveal },
  { id: "settings:library-metadata-browse", label: msg("Choose boat metadata directory"), description: msg("Choose boat metadata directory"), keywords, topic: "settings", reveal },
  { id: "settings:library-save", label: msg("Save library settings"), description: msg("Save the library's folders and scraping choices."), keywords, topic: "settings", reveal },
  { id: "settings:library-db-host", label: msg("Database host"), description: msg("The SYRF PostgreSQL server the boat metadata is downloaded from, read only."), keywords: dbKeywords, topic: "settings", reveal },
  { id: "settings:library-db-port", label: msg("Database port"), description: msg("The SYRF PostgreSQL server the boat metadata is downloaded from, read only."), keywords: dbKeywords, topic: "settings", reveal },
  { id: "settings:library-db-name", label: msg("Database name"), description: msg("The SYRF PostgreSQL server the boat metadata is downloaded from, read only."), keywords: dbKeywords, topic: "settings", reveal },
  { id: "settings:library-db-user", label: msg("Database user"), description: msg("The SYRF PostgreSQL server the boat metadata is downloaded from, read only."), keywords: dbKeywords, topic: "settings", reveal },
  { id: "settings:library-db-password", label: msg("Database password"), description: msg("The SYRF PostgreSQL server the boat metadata is downloaded from, read only."), keywords: [...dbKeywords, msg("credentials")], topic: "settings", reveal },
  { id: "settings:library-db-tls", label: msg("Verify TLS certificate"), description: msg("The SYRF PostgreSQL server the boat metadata is downloaded from, read only."), keywords: [...dbKeywords, "TLS", "SSL"], topic: "settings", reveal },
  { id: "settings:library-db-test", label: msg("Test connection"), description: msg("Test the connection and required database tables"), keywords: dbKeywords, topic: "settings", reveal },
  { id: "settings:library-db-download", label: msg("Download boat metadata"), description: msg("Downloading reads the supported trackers' boats, races and track records into the boat metadata. Nothing is written to the database, and races saved by scraping stay."), keywords: [...dbKeywords, msg("export"), msg("metadata")], topic: "settings", reveal },
  { id: "settings:library-db-cancel", label: msg("Cancel download"), description: msg("Cancel download"), keywords: dbKeywords, topic: "settings", reveal },
  { id: "tracks:boat-search", label: msg("Search tracks by vessel details"), description: msg("Search all vessel fields, including name, model, class, make and builder"), keywords: ["SYRF", "GeoJSON", "boat name", "model", "class", "make", "builder", "vessel", "sail number"], topic: "tracks", reveal: ["panel:left", "section:tracks"] },
  { id: "tracks:boat-import", label: msg("Import this boat track; weather can be fetched afterwards"), description: msg("Import this boat track; weather can be fetched afterwards"), keywords: ["SYRF", "GeoJSON"], topic: "tracks", reveal: ["panel:left", "section:tracks"], landing: "tracks:boat-search" },
  { id: "tracks:boat-more", label: msg("More boat tracks"), description: msg("The next hundred tracks of the search; they load on their own as the list is scrolled to its end."), keywords: [msg("page"), msg("scroll"), msg("library")], topic: "tracks", reveal: ["panel:left", "section:tracks"], landing: "tracks:boat-search" },
];
export default features;
