export type OrderType = 'eat_in' | 'takeaway' | 'click_and_collect' | 'delivery' | 'drive'
export type StationStatus = 'new' | 'in_progress' | 'ready' | 'held' | 'assembled' | 'served'
export type LineType = 'item' | 'combo_component' | 'modifier' | 'comment'
export type StationRole = 'preparation' | 'holding' | 'assembly' | 'ready_board'

export interface TimerThresholds {
  warning_secs: number
  critical_secs: number
}

export interface KdsLine {
  line_id: string
  product_name: string
  quantity: number
  parent_line_id: string | null
  line_type: LineType
  comment: string | null
  acknowledged: boolean
}

export interface KdsOrderPayload {
  order_id: string
  station_id: string
  order_number_short: string
  external_order_id: string | null
  channel: string
  order_type: OrderType
  customer_name: string | null
  stage: string
  lines: KdsLine[]
  station_statuses: Record<string, StationStatus>
  arrived_at: number
  timer_thresholds: TimerThresholds
}

export interface KdsOrderUpdate {
  order_id: string
  status: string
  stage: string
  station_statuses: Record<string, StationStatus>
}

export interface KdsAckPayload {
  order_id: string
  station_id: string
  line_id: string | null
}

/** État d'une commande enrichi côté client (status ajouté par les events order_updated). */
export interface TrackedOrder extends KdsOrderPayload {
  status: string
}

export interface Station {
  id: string
  name: string
  role: StationRole
  temperature_group: string | null
  output_type: string
  sort_order: number
  enabled: boolean
}

export interface KdsConfig {
  active_profile: string
}
