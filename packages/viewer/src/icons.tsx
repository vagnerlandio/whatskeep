// Lucide SVG icons (ISC), embedded in the bundle: nothing is fetched from the network.
// Always use these instead of emojis, which look different on every system.
import {
  Archive,
  ArrowLeft,
  ArrowLeftRight,
  Ban,
  CalendarDays,
  Camera,
  Check,
  CloudDownload,
  ChevronDown,
  CircleAlert,
  CircleCheck,
  ChevronRight,
  Contact,
  Download,
  FileText,
  Folder,
  HardDriveDownload,
  Image,
  Info,
  Headphones,
  ImagePlay,
  KeyRound,
  Languages,
  Lock,
  LockKeyhole,
  LogOut,
  type LucideIcon,
  MapPin,
  Megaphone,
  MessageSquareText,
  Mic,
  Pause,
  Pin,
  PinOff,
  Play,
  Search,
  ShieldCheck,
  Star,
  Sticker,
  Trash2,
  TriangleAlert,
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
  checkCircle: CircleCheck,
  chevronRight: ChevronRight,
  chevronDown: ChevronDown,
  chats: MessageSquareText,
  close: X,
  cloud: CloudDownload,
  contact: Contact,
  document: FileText,
  download: Download,
  folder: Folder,
  gif: ImagePlay,
  group: Users,
  headphones: Headphones,
  image: Image,
  import: HardDriveDownload,
  info: Info,
  key: KeyRound,
  languages: Languages,
  lock: Lock,
  lockKeyhole: LockKeyhole,
  logOut: LogOut,
  location: MapPin,
  mic: Mic,
  pause: Pause,
  pin: Pin,
  pinOff: PinOff,
  play: Play,
  search: Search,
  shield: ShieldCheck,
  star: Star,
  sticker: Sticker,
  trash: Trash2,
  swap: ArrowLeftRight,
  video: Video,
  warning: TriangleAlert,
} satisfies Record<string, LucideIcon>;

export type IconName = keyof typeof icons;

/** Stroke icon in `currentColor`. `fill` fills closed shapes (star, play). */
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
