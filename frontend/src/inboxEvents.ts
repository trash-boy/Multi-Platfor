import type { UnreadCounts } from './api'

/** Sidebar badge sync: pages that learn a fresh unread count broadcast it. */
export const UNREAD_EVENT = 'inbox-unread'

export function publishUnread(u: UnreadCounts) {
  window.dispatchEvent(new CustomEvent<number>(UNREAD_EVENT, { detail: u.total }))
}
