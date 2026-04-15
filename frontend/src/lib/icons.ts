import {
  Database, Layers, Server, Globe, Cog, Clock, Box, Folder,
  Shield, Cloud, HardDrive, Cpu, Network, FileText, Zap,
  Calendar, ArrowLeftRight, Search, Workflow, Container,
  Puzzle, Radio, Activity, Terminal, MonitorDot, Key,
  Lock, Unlock, AlertTriangle, CheckCircle, XCircle,
  type LucideIcon,
} from 'lucide-react';

/**
 * Map of icon name (lowercase) → Lucide icon component.
 * Used by ComponentNode, ComponentPalette, and the catalog system
 * to resolve icon strings from the backend into React components.
 */
export const ICON_MAP: Record<string, LucideIcon> = {
  // Core icons (match builtin component types)
  database: Database,
  layers: Layers,
  server: Server,
  globe: Globe,
  cog: Cog,
  clock: Clock,
  box: Box,
  folder: Folder,

  // Extended icons
  shield: Shield,
  cloud: Cloud,
  'hard-drive': HardDrive,
  harddrive: HardDrive,
  cpu: Cpu,
  network: Network,
  'file-text': FileText,
  filetext: FileText,
  zap: Zap,
  calendar: Calendar,
  'arrow-left-right': ArrowLeftRight,
  arrowleftright: ArrowLeftRight,
  search: Search,
  workflow: Workflow,
  container: Container,
  puzzle: Puzzle,
  radio: Radio,
  activity: Activity,
  terminal: Terminal,
  monitor: MonitorDot,
  key: Key,
  lock: Lock,
  unlock: Unlock,
  'alert-triangle': AlertTriangle,
  alerttriangle: AlertTriangle,
  'check-circle': CheckCircle,
  checkcircle: CheckCircle,
  'x-circle': XCircle,
  xcircle: XCircle,

  // PascalCase aliases (for backward compat with COMPONENT_TYPE_ICONS values)
  Database: Database,
  Layers: Layers,
  Server: Server,
  Globe: Globe,
  Cog: Cog,
  Clock: Clock,
  Box: Box,
  Folder: Folder,
  Shield: Shield,
  Cloud: Cloud,
  HardDrive: HardDrive,
  Cpu: Cpu,
  Network: Network,
  FileText: FileText,
  Zap: Zap,
  Calendar: Calendar,
  ArrowLeftRight: ArrowLeftRight,
  Search: Search,
  Workflow: Workflow,
  Container: Container,
  Puzzle: Puzzle,
  Radio: Radio,
  Activity: Activity,
  Terminal: Terminal,
  MonitorDot: MonitorDot,
  Key: Key,
  Lock: Lock,
};

/** Resolve an icon name to a Lucide component, with Box as fallback. */
export function resolveIcon(iconName: string | null | undefined): LucideIcon {
  if (!iconName) return Box;
  return ICON_MAP[iconName] || ICON_MAP[iconName.toLowerCase()] || Box;
}
