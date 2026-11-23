#include "side.h"
// digest §4.3 "Names a program reads": an unnamed local type has no name
// to print; a local class beside it renamed, unobserved, is equal.
namespace std { class type_info { public: const char *name() const noexcept; }; }
#ifdef B
const char *f() { struct Y {}; struct { int m; } u; return typeid(u).name(); }
#else
const char *f() { struct X {}; struct { int m; } u; return typeid(u).name(); }
#endif
