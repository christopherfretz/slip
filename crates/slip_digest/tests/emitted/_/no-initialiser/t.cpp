#include "side.h"
// WP-155 fix round (B2, rules 1 and 3): no initialiser. A class (or an
// array of one) whose default constructor is trivial and destructor is
// trivial is not marked; a user-provided default constructor, or a
// default member initialiser, is a non-trivial one and is marked.
struct K {};
struct T2 { int x; };
struct U { U() {} };
struct M { int x = 1; };
K k;
K ka[3];
T2 t2;
thread_local K tk;
U u;
M m;
template <class T> struct Tr { static K r; static U s; };
template <class T> K Tr<T>::r;
template <class T> U Tr<T>::s;
#ifndef B
K *pr = &Tr<int>::r;
U *ps = &Tr<int>::s;
#endif
