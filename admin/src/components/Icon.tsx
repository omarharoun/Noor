import {
  LayoutDashboard,
  ArrowLeftRight,
  Store,
  Landmark,
  Wallet,
  Webhook,
  Search,
  Bell,
  Settings,
  TrendingUp,
  CheckCircle2,
  Clock,
  ArrowRight,
  Plus,
  Download,
  Zap,
  Check,
  Receipt,
  ExternalLink,
  Circle,
  Loader,
  RotateCw,
  Eye,
  Percent,
  Link as LinkIcon,
  Users,
  Activity,
  LogOut,
  PlusCircle,
  Send,
  List,
  FileText,
  Moon,
  Sun,
  type LucideIcon,
} from 'lucide-react';

// Kebab-case names (as used in the design kit) → Lucide components.
const REGISTRY: Record<string, LucideIcon> = {
  'layout-dashboard': LayoutDashboard,
  'arrow-left-right': ArrowLeftRight,
  store: Store,
  landmark: Landmark,
  wallet: Wallet,
  webhook: Webhook,
  search: Search,
  bell: Bell,
  settings: Settings,
  'trending-up': TrendingUp,
  'check-circle-2': CheckCircle2,
  clock: Clock,
  'arrow-right': ArrowRight,
  plus: Plus,
  download: Download,
  zap: Zap,
  check: Check,
  receipt: Receipt,
  'external-link': ExternalLink,
  circle: Circle,
  loader: Loader,
  'rotate-cw': RotateCw,
  eye: Eye,
  percent: Percent,
  link: LinkIcon,
  users: Users,
  activity: Activity,
  'log-out': LogOut,
  'plus-circle': PlusCircle,
  send: Send,
  list: List,
  'file-text': FileText,
  moon: Moon,
  sun: Sun,
};

interface IconProps {
  name: string;
  size?: number;
  stroke?: number;
  color?: string;
  className?: string;
  style?: React.CSSProperties;
}

export function Icon({ name, size = 16, stroke = 1.75, color, className, style }: IconProps) {
  const Cmp = REGISTRY[name] ?? Circle;
  return (
    <span className={`ic ${className ?? ''}`} style={{ display: 'inline-flex', color, ...style }}>
      <Cmp size={size} strokeWidth={stroke} />
    </span>
  );
}
