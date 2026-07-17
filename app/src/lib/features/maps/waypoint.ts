/**
 * The in-game waypoint string: `/wp [<Planet>,<Lon>,<Lat>,<Alt>,<Label>]`,
 * paste-ready for the game chat. The string is a contract with a parser
 * we do not control, so it is built in exactly one place, integer-
 * rounded, with the planet name data-driven from the bundled catalogue
 * (`technicalName`) and the user label sanitised against the delimiter
 * set. A planet without a technical name (Thule) cannot form a waypoint;
 * callers disable the action rather than copy a string that cannot work.
 */

/** The label with the waypoint delimiters neutralised: brackets are
 * dropped, commas soften to semicolons, whitespace collapses. */
export function sanitiseWaypointLabel(label: string): string {
	return label
		.replaceAll('[', '')
		.replaceAll(']', '')
		.replaceAll(',', ';')
		.replace(/\s+/g, ' ')
		.trim();
}

/** The paste-ready waypoint string, or null when the planet has no
 * technical name to address it by. */
export function formatWaypoint(args: {
	technicalName: string | null;
	lon: number;
	lat: number;
	altitude: number | null;
	label: string;
}): string | null {
	if (!args.technicalName) return null;
	const lon = Math.round(args.lon);
	const lat = Math.round(args.lat);
	const alt = Math.round(args.altitude ?? 0);
	const label = sanitiseWaypointLabel(args.label);
	return `/wp [${args.technicalName},${lon},${lat},${alt},${label}]`;
}
