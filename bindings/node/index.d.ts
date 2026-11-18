import type { DrawOptions, ExportFiles, Group, Map, SvgFile } from './native'

export type { DrawOptions, ExportFiles, Group, Map, SvgFile }

/** Draw a profile at a place: a file (`profile`), a script's text (`script`), or the built-in postcard. */
export declare function draw(options?: DrawOptions): Promise<Map>
/** The graph a profile builds, as an object; no map data is downloaded. */
export declare function graph(options?: DrawOptions): Record<string, unknown>
/** The command line, `drawland map …`, on `argv` (without the program's name); the exit code. */
export declare function cli(argv: string[]): number
/** The built-in postcard's script: the profile drawn when none is given. */
export declare const POSTCARD: string
