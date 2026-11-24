#include "side.h"
// WP-124 item 4: changed in a template nothing instantiates; the AST holds it.
template<class> struct B1 { };
template<class> struct B2 { };
#ifdef B
template<class... Ts> struct V : B2<Ts>... { };
#else
template<class... Ts> struct V : B1<Ts>... { };
#endif
