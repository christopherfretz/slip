#include "side.h"
// digest-kinds §11.9 (WP-045 decision 4): the use of a static data member
// makes Kiln instantiate its out-of-line definition as a new declaration
// with the pattern definition's attributes and specifiers; the member's
// own unit reads its first declaration and leaves that one out.
template <class T> struct A { static int s; };
template <class T> [[maybe_unused]] constinit int A<T>::s = 1;
A<int> a;
#ifdef B
int y() { return A<int>::s; }
#endif
