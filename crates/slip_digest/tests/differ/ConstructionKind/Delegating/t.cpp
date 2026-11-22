#include "side.h"
// WP-035, digest-kinds §7 `ConstructionKind` `Delegating`.
#ifdef B
struct D { D(int); int m; D() : m(1) {} };
#else
struct D { D(int); D() : D(1) {} };
#endif
