#include "side.h"
// WP-124 item 4: changed in a template nothing instantiates; the AST holds it.
template<class T> struct S { S(T); };
#ifdef B
template<class T> S(T) -> S<long>;
#else
template<class T> S(T) -> S<int>;
#endif
