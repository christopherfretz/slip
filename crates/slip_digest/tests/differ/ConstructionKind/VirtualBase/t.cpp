#include "side.h"
// WP-035, digest-kinds §7 `ConstructionKind` `VirtualBase`.
struct B1 { B1(int); B1(long); };
#ifdef B
struct D : B1 { D() : B1(1) {} };
#else
struct D : virtual B1 { D() : B1(1) {} };
#endif
