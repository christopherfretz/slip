#include "side.h"
// WP-136 fix round 1 (M12): a generic lambda whose `operator()<int>` fails
// substitution under a SFINAE probe (`has_call`) on side A only. The closure's
// local member list (`decl_class::local_member`) must skip the failed
// `operator()<int>` so `make_l()` is unchanged and determinate.
auto make_l() {
  return [](auto t) -> decltype(t.size()) { return 0; };
}
template <class L> constexpr bool has_call = requires(L l) { l(1); };
int u() {
#ifdef B
  return 1;
#else
  return has_call<decltype(make_l())> ? 1 : 0;
#endif
}
