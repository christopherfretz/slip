#include "side.h"
// WP-124 item 4 (Kiln WP-3027 A6 partial_def_head): the definition's constraint differs from the declaration's, so it is another partial specialisation (a default template argument on a partial specialisation is ill-formed).
template<class T> concept C = sizeof(T) == 4;
template<class T, class U> struct P;
template<class T> requires C<T> struct P<T, int*>;
#ifdef B
template<class T> requires (!C<T>) struct P<T, int*> { T m; };
#else
template<class T> requires C<T> struct P<T, int*> { T m; };
#endif
