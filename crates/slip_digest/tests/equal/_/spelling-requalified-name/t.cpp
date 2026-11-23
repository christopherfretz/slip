#include "side.h"
// §4.4 "Spellings", rule 10: a qualifier is not a fact: requalifying a resolved name that names the same entity, in a body, changes no digest (WP-052's sweep and the namespace migration depend on it).
namespace n { struct S { static constexpr int v = 1; }; int h(int); }
using n::S;
using n::h;
#ifdef B
template<class T> int g(T) { return ::n::S::v + n::h(1); }
#else
template<class T> int g(T) { return S::v + h(1); }
#endif
