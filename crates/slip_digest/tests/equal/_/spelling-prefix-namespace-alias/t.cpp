#include "side.h"
// §4.4 "Spellings", rule 10 and rule 11: a namespace alias against the namespace, in a qualified name in a body, in an unresolved call's qualifier and in a using-declaration: a namespace level holds nothing beyond the namespace, so the digests are equal.
namespace n { struct S { static constexpr int v = 1; }; int h(int); template<class T> int k(T); }
namespace m = n;
#ifdef B
using m::h;
template<class T> int g(T t) { return m::S::v + m::k(t); }
#else
using n::h;
template<class T> int g(T t) { return n::S::v + n::k(t); }
#endif
