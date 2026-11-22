#include "side.h"
// §5.5 RecordData: a use that makes Kiln declare and define the implicit members of C changes no declared unit (digest §2.2).
#ifdef B
struct C { int a; virtual void f(); }; void use() { C x; C y = x; y = x; C z = static_cast<C&&>(y); }
#else
struct C { int a; virtual void f(); };
#endif
