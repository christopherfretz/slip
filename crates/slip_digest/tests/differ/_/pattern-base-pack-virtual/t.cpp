#include "side.h"
// WP-124 item 4: changed in a template nothing instantiates; the AST holds it.
#ifdef B
template<class... Ts> struct V : virtual Ts... { };
#else
template<class... Ts> struct V : Ts... { };
#endif
