# Data sources and acknowledgements

PolarExplorer embeds the ORC catalogue and Natural Earth map. Weather and
tracker data are requested by the user. Sampled environmental values retain
their dataset identifier and version in the project. These notices describe
the sources used by this release; they do not change the providers' terms.

## ORC catalogue

The embedded catalogue is built from [jieter/orc-data](https://github.com/jieter/orc-data).
Copyright (c) 2015, 2016, 2017 Jan Pieter Waagmeester. Licensed under MIT; the
complete [copyright and permission notice](licenses/orc-data-MIT.txt) accompanies
the application. The native About panel records the embedded catalogue's
commit, source date, build date and certificate count. Its source polars are
based on ORC certificates; inclusion does not imply endorsement by ORC.

Settings → **ORC polars** can add the current year's certificates from
[ORC's public data service](https://data.orc.org/public/WPub.dll), the origin
of that data: every listed country's valid certificates, read the same way and
kept on this computer beside the embedded catalogue, each certificate once.
Nothing is downloaded unless you start it there or choose a schedule for it.
Certificates and their contents remain ORC's and the rating offices'.

## ORR catalogue

Complete public certificates, ratings and offshore/short-course polar tables
come from [RegattaMan’s ORR valid list](https://www.regattaman.com/valid_list_ora.php?crule=ORR&sdir=true&ssdir=true&sort=3&ssort=0).
The bundled 2026 snapshot contains 300 certificates and 600 table variants,
captured 2026-09-30. Settings can refresh a chosen year. The snapshot preserves
all named list columns and certificate fields, public ownership and comments,
hull/rig/sail/stability/trim measurements, performance metrics, drawing
parameters, every embedded rating block, and original speed/time-allowance
tables. Rating values come from the page's embedded JSON, not its initially
empty display inputs. Only normalized certificate data is stored, not raw HTML,
page scripts or login/session state.
See `assets/orr/README.md` for the snapshot’s provenance and rebuild command.

## Weather: archives listed by Hindsight Sync

PolarExplorer reads weather from one reanalysis archive, chosen in Settings →
**Reanalysis** among those that [Hindsight Sync](https://hindsight-sync.whirlwindai.com),
a separate application on this computer, lists: in the cloud, read with the
credentials Hindsight Sync hands out, or in a folder on this computer.
PolarExplorer asks Hindsight Sync's read-only API on `127.0.0.1` which
archives exist and reads only the one chosen. Each track's samples record
the archive's identity and layout version. The archives hold wind, waves and
current (with tide) on one hourly 0.25° grid; Hindsight Sync builds them from
the upstream data below.

### Wind and waves: ERA5

Contains modified Copernicus Climate Change Service information. ERA5 is
produced by ECMWF for the Copernicus Climate Change Service (C3S). The
archives are built from it and PolarExplorer interpolates their values to
track positions and converts units; these derived samples are not an
official ECMWF product.

- [ERA5 hourly data on single levels](https://cds.climate.copernicus.eu/datasets/reanalysis-era5-single-levels?tab=overview)
  provides the underlying wind and wave data, dataset documentation and licence.
- [ARCO-ERA5](https://github.com/google-research/arco-era5), hosted on Google
  Cloud, is the copy of ERA5 Hindsight Sync reads it from. The archive
  software's licence does not replace the underlying ERA5 data terms.

Neither the European Commission nor ECMWF is responsible for PolarExplorer'
use of Copernicus information or for its derived results.

### Currents: Copernicus Marine

Generated using E.U. Copernicus Marine Service Information. The archives'
surface current, tide included, is Copernicus Marine's GlobCurrent total
current, product `MULTIOBS_GLO_PHY_MYNRT_015_003` (multiyear and
near-real-time, DOI [10.48670/mds-00327](https://doi.org/10.48670/mds-00327)).
Its product page provides its scientific references and licence. When
sharing derived work, cite that product and retain the sample provenance.
See the service's
[citation guidance](https://help.marine.copernicus.eu/en/articles/4444611-citing-copernicus-marine-products-and-services).

## Basemap and imported tracks

The bundled land and coastline geometry comes from [Natural Earth](https://www.naturalearthdata.com/about/terms-of-use/),
whose map data are in the public domain. No map tiles are downloaded.

YellowBrick, Geovoile and Blue Water Tracks provide user-selected race tracks.
Their event data and any files you import remain subject to the rights and
terms of their respective providers. PolarExplorer' application licence does
not grant rights to third-party event data.

The optional SYRF track library is a boat metadata file and a folder of
individual-track GeoJSON files, chosen in Settings, that are searched and
imported from. The scraper fills them with finished races only. The boat
metadata can also be downloaded, read-only and only on request, from a SYRF
PostgreSQL database the user names in Settings; nothing is written to it. Their formats follow
[syrf-schema](https://github.com/sailing-yacht-research-foundation/syrf-schema)
and the individual-track formatter in
[raw-data-server](https://github.com/sailing-yacht-research-foundation/raw-data-server).
Original race URLs and source identifiers accompany searchable tracks.
Scraping uses public YellowBrick, Geovoile and Blue Water tracks in Rust;
it does not run the upstream Node or browser scrapers. YellowBrick catalogue
discovery uses configured, user-authorized mobile credentials to resolve race
codes and associate only products explicitly listed as free. Credentials are
local settings, never bundled into builds, logged or included in metadata.
The metadata is limited to the selected provider families.
