#include "side.h"
// digest §4.3 "Names a program reads": `typeid(X).name()` prints the
// local class's name; renaming it changes f.
namespace std { class type_info { public: const char *name() const noexcept; }; }
#ifdef B
const char *f() { struct Y {}; return typeid(Y).name(); }
#else
const char *f() { struct X {}; return typeid(X).name(); }
#endif
