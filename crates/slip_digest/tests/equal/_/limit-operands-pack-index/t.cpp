// std: c++26
#include "side.h"
// A limit by name (spec/digest.md §2.3, WP-156; WP-153 §13.2 B7 item 3):
// a pack index after substitution is absent at the pin (the field's type
// is the selected type), so a name in the index moves unseen in the
// instantiated field: equal. Site 5 is reserved.
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
template <class... T> struct P { T...[sizeof(make(x)) - 4] f; };
P<lib::X, long> p;
