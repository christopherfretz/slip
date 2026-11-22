#include "side.h"
// WP-035, digest-kinds §7 `ConstructionKind` `Complete`.
struct B1 { B1(int); B1(long); };
#ifdef B
struct D : B1 { D() : B1(1) {} };
#else
struct D { B1 m; D() : m(1) {} };
#endif
