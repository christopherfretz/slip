#include "side.h"
// digest-kinds §4.6 ArgView `Expr` (WP-123, form B): a befriended
// specialisation's unconverted argument is written by its profile, but a
// destructor name in it is still a left-out spelling
// (`spelling/destructor-name`), an older cause than WP-123's two.
struct D { ~D(); };
template <class T, auto I> struct It;
template <class T> struct M { friend struct It<T, sizeof(((D *)nullptr)->~D(), 0)>; };
int g() { return 0; }
