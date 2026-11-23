#include "side.h"
// digest §4.3: a closure whose lambda-expression only a type holds (a
// template argument) is not found by its owner's numbering (x's stream
// holds no initialiser): it has no key, and what names it is
// indeterminate (`UndeclaredLocal`).
template <class T> struct X {};
X<decltype([] {})> x;
