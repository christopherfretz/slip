#include "side.h"
// §5.7 ColdDeclData `subst_failed` (unsupported, WP-136; Kiln WP-3281): a class template specialisation whose partial specialisation is ambiguous, named only through a pointer, so no error is raised; Kiln marks it `ClassPartialAmbiguity` on an invalid declaration.
template <class T, class U> struct X;
template <class T> struct X<T, int> {};
template <class U> struct X<int, U> {};
X<int, int> *p;
