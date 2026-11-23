#include "side.h"
// WP-124 item 4 (Kiln WP-3027 A6 partial_def_head): as pattern-partial-parameter-name, the definition's body naming the renamed parameter and the specialisation used.
template<class T, class U> struct P;
template<class T> struct P<T, int*>;
#ifdef B
template<class Z> struct P<Z, int*> { Z m; using type = Z; };
#else
template<class T> struct P<T, int*> { T m; using type = T; };
#endif
P<long, int*>::type x = 0;
