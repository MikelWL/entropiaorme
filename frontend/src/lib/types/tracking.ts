import type { Ped, Pes, Seconds, ISODate, Ratio, NotableEventCategory, NotableEventType } from './common';

/** A tracking session summary (shown in session history list) */
export interface TrackingSession {
	id: string;
	startTime: ISODate;
	endTime: ISODate | null;
	duration: Seconds;
	primaryMobs: string[];
	primaryWeapons: string[];
	cost: Ped;
	returns: Ped;
	net: Ped;
	returnRate: Ratio;
	globals: number;
	hofs: number;
}

export interface CostBreakdown {
	weaponCost: Ped;
	healCost: Ped;
	enhancerCost: Ped;
	armourCost: Ped;
}

/** Expanded session detail (inline expand from history row) */
export interface SessionDetail {
	sessionId: string;
	summary: {
		cost: Ped;
		returns: Ped;
		pes: Pes;
		net: Ped;
		returnRate: Ratio;
		kills: number;
		duration: Seconds;
		costBreakdown?: CostBreakdown;
	};
	notableEvents: NotableEvent[];
	lootBreakdown: LootItem[];
	effectiveLoot: Ped;
	toolStats: ToolStat[];
	skillGains: SkillGain[];
}

export interface NotableEvent {
	type: NotableEventCategory;
	eventType: NotableEventType;
	target: string;
	item: string;
	value: Ped;
}

export interface LootItem {
	name: string;
	quantity: number;
	ttValue: Ped;
}

export interface ToolStat {
	weaponName: string;
	shotsFired: number;
	damageDealt: number;
	crits: number;
	costAttributed: Ped;
}

export interface SkillGain {
	skillName: string;
	level: number;
	ttValueGained: Ped;
}
