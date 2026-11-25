#include "side.h"
// WP-156 fix round 2, S-2 (the review's M12): the fixed underlying type of
// a member enumeration of a class template, instantiated: its definition is
// its resolved unit `definition`, whose operand unit reads it
// (`decl_class.rs` `enum_definition`).
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
template <class T> struct CE { enum E : decltype(make(x)) { e }; };
int ev = CE<int>::e;
