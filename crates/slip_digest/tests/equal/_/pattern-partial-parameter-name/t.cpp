#include "side.h"
// WP-124 item 4 (Kiln WP-3027 A6 partial_def_head): a partial specialisation defined with its parameter renamed; the name is not part of the entity.
template<class T, class U> struct P;
template<class T> struct P<T, int*>;
#ifdef B
template<class Z> struct P<Z, int*> { Z m; };
#else
template<class T> struct P<T, int*> { T m; };
#endif
