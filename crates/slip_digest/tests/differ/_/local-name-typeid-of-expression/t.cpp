#include "side.h"
// digest §4.3 "Names a program reads": `typeid(e)` of an expression whose
// static type is a pointer to a local class prints the class's name.
namespace std { class type_info { public: const char *name() const noexcept; }; }
#ifdef B
const char *f() { struct Y {} y; return typeid(&y).name(); }
#else
const char *f() { struct X {} x; return typeid(&x).name(); }
#endif
