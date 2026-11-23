#include "side.h"
// WP-124 item 4: changed in a template nothing instantiates; the AST holds it.
#ifdef B
template<class... Ts> int f(Ts... xs) { return (xs - ...); }
#else
template<class... Ts> int f(Ts... xs) { return (... - xs); }
#endif
