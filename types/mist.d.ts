declare module 'mist' {
  export interface Box<T> {
    value: T
  }

  export interface ReadonlyBox<T> {
    readonly value: T
  }

  export interface StoreOptions<T> {
    /** wx storage key — hydrates on creation, writes debounced (~200ms) on mutation */
    persist?: string
    /** bump when the persisted shape changes; unmatched saves go through `migrate` */
    version?: number
    migrate?: (old: unknown, oldVersion: number) => T
  }

  export function state<T>(init: T): Box<T>
  export function derived<T>(compute: () => T): ReadonlyBox<T>
  export function store<T>(init: T, opts?: StoreOptions<T>): Box<T>
  /**
   * Callback props (`onXxx`) do not appear in `defaults`. Give an explicit
   * generic for full typing: `props<{ todo: Todo; onToggle(id: number): void }>()`.
   * Without the generic the result is an open record.
   */
  export function props<T extends object = Record<string, any>>(defaults?: object): T

  export function onLaunch(fn: (options?: Record<string, unknown>) => void): void

  /** app-only */
  export function onError(fn: (error: string) => void): void
  /** app-only */
  export function onPageNotFound(fn: (e: { path: string; query: Record<string, string>; isEntryPage: boolean }) => void): void
  /** app-only */
  export function onUnhandledRejection(fn: (e: { reason: unknown; promise: Promise<unknown> }) => void): void
  /** app-only */
  export function onThemeChange(fn: (e: { theme: 'light' | 'dark' }) => void): void

  export function onLoad(fn: (query: Record<string, string>) => void | Promise<void>): void
  export function onShow(fn: () => void): void
  export function onReady(fn: () => void): void
  export function onHide(fn: () => void): void
  export function onUnload(fn: () => void): void

  /** component-only; runs before properties/data — state writes are M1017 */
  export function onCreate(fn: () => void): void
  export function onAttach(fn: () => void): void
  export function onDetach(fn: () => void): void
  /** component-only */
  export function onMove(fn: () => void): void
  /** component-only — maps to `pageLifetimes.show` */
  export function onPageShow(fn: () => void): void
  /** component-only — maps to `pageLifetimes.hide` */
  export function onPageHide(fn: () => void): void

  /** page-only */
  export function onPullDownRefresh(fn: () => void | Promise<void>): void
  /** page-only */
  export function onReachBottom(fn: () => void | Promise<void>): void
  /** page-only — fires every scroll frame; keep the handler cheap */
  export function onPageScroll(fn: (e: { scrollTop: number }) => void): void
  /** page-only; WeChat delivers `index` as a string */
  export function onTabItemTap(
    fn: (item: { index: string; pagePath: string; text: string }) => void
  ): void
  /** pages, and components via `pageLifetimes.resize` */
  export function onResize(fn: (e: { size: { windowWidth: number; windowHeight: number } }) => void): void
  /** page-only */
  export function onRouteDone(fn: () => void): void
  /** page-only */
  export function onSaveExitState(fn: () => { data: unknown; expireTimeStamp?: number }): void

  export interface ShareAppMessage {
    title?: string
    path?: string
    imageUrl?: string
    promise?: Promise<ShareAppMessage>
  }
  export interface ShareTimeline {
    title?: string
    query?: string
    imageUrl?: string
  }
  export interface FavoritesConfig {
    title?: string
    imageUrl?: string
    query?: string
  }
  export function onShareAppMessage(
    fn: (e?: { from: 'button' | 'menu'; target?: unknown }) => ShareAppMessage
  ): void
  export function onShareTimeline(fn: () => ShareTimeline): void
  export function onAddToFavorites(fn: (e?: { webviewUrl?: string }) => FavoritesConfig): void
}
