#include "side.h"
// digest §4.3 "Names a program reads": `typeid(X::I).name()` prints the
// local class around I; renaming it changes f.
namespace std { class type_info { public: const char *name() const noexcept; }; }
#ifdef B
const char *f() { struct Y { struct I {}; }; return typeid(Y::I).name(); }
#else
const char *f() { struct X { struct I {}; }; return typeid(X::I).name(); }
#endif
