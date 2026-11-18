import type { DrawOptions, ExportFiles, Group, Map, SvgFile } from './native'
import type { Library } from './library'

export type { DrawOptions, ExportFiles, Group, Map, SvgFile, Library }

/**
 * A graph built in Node, with the same library a profile calls: `g.features`,
 * `g.areas`, `g.lines`, `g.fills`, `g.modules`, `g.compose`, `g.page`,
 * `g.world`, `g.output(…)`, `g.draw(…)`, `g.view(…)`. Draw it with
 * `drawland.draw({ profile: g })`.
 */
export declare class Graph {
  /** `lat`/`lng`: the place `world` answers for, over `output`'s; `dir`: where views read the styles they name. */
  constructor(options?: { lat?: number; lng?: number; dir?: string })
  /** The graph built, as the engine reads it. The graph is finished. */
  finish(): Record<string, unknown>
}
export interface Graph extends Library {}

export interface Draw extends Omit<DrawOptions, 'profile' | 'graph'> {
  /** A profile file (`.js`, `.ts`, `.json`) or a `Graph` built here. */
  profile?: string | Graph
}

/** Draw a profile at a place: a file or a `Graph` (`profile`), a script's text (`script`), or the built-in postcard. */
export declare function draw(options?: Draw): Promise<Map>
/** The graph a profile builds, as an object; no map data is downloaded. */
export declare function graph(options?: Draw): Record<string, unknown>
/** The command line, `drawland map …`, on `argv` (without the program's name); the exit code. */
export declare function cli(argv: string[]): number
/** The built-in postcard's script: the profile drawn when none is given. */
export declare const POSTCARD: string
/** Every function of the library, as a script names it (`areas.union`). */
export declare const FUNCTIONS: readonly string[]
