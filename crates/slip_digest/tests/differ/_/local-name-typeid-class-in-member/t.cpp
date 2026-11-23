#include "side.h"
// digest §4.3 "Names a program reads": `typeid(Z)` of a local class of a
// member of the local class X prints both names; renaming X changes f.
namespace std { class type_info { public: const char *name() const noexcept; }; }
#ifdef B
const char *f() { struct Y { const char *g() { struct Z {}; return typeid(Z).name(); } }; return Y().g(); }
#else
const char *f() { struct X { const char *g() { struct Z {}; return typeid(Z).name(); } }; return X().g(); }
#endif
