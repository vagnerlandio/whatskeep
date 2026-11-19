// Ícones SVG do Lucide (ISC), embutidos no bundle: nada é buscado na rede.
// Use sempre estes em vez de emojis, que mudam de aparência em cada sistema.
import {
  Archive,
  ArrowLeft,
  ArrowLeftRight,
  Ban,
  CalendarDays,
  Camera,
  Check,
  ChevronDown,
  CircleAlert,
  Contact,
  Download,
  FileText,
  Folder,
  HardDriveDownload,
  Headphones,
  ImagePlay,
  KeyRound,
  Languages,
  Lock,
  LockKeyhole,
  type LucideIcon,
  MapPin,
  Megaphone,
  MessageSquareText,
  Mic,
  Pause,
  Play,
  Search,
  ShieldCheck,
  Star,
  Sticker,
  User,
  Users,
  Video,
  X,
} from "lucide-react";

export const icons = {
  account: User,
  alert: CircleAlert,
  archive: Archive,
  back: ArrowLeft,
  broadcast: Megaphone,
  calendar: CalendarDays,
  camera: Camera,
  cancel: Ban,
  check: Check,
  chevronDown: ChevronDown,
  chats: MessageSquareText,
  close: X,
  contact: Contact,
  document: FileText,
  download: Download,
  folder: Folder,
  gif: ImagePlay,
  group: Users,
  headphones: Headphones,
  import: HardDriveDownload,
  key: KeyRound,
  languages: Languages,
  lock: Lock,
  lockKeyhole: LockKeyhole,
  location: MapPin,
  mic: Mic,
  pause: Pause,
  play: Play,
  search: Search,
  shield: ShieldCheck,
  star: Star,
  sticker: Sticker,
  swap: ArrowLeftRight,
  video: Video,
} satisfies Record<string, LucideIcon>;

export type IconName = keyof typeof icons;

/** Ícone de traço em `currentColor`. `fill` preenche formas fechadas (estrela, play). */
export function Icon({
  name,
  size = 24,
  className,
  title,
  strokeWidth = 1.75,
  fill,
}: {
  name: IconName;
  size?: number;
  className?: string;
  title?: string;
  strokeWidth?: number;
  fill?: boolean;
}) {
  const C = icons[name];
  return (
    <C
      className={className ? `icon ${className}` : "icon"}
      size={size}
      strokeWidth={strokeWidth}
      fill={fill ? "currentColor" : "none"}
      aria-hidden={title ? undefined : true}
      aria-label={title}
      role={title ? "img" : undefined}
    />
  );
}
