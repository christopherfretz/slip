#include "side.h"
// WP-035, digest-kinds §7 `ConstructInfo` `zeroing`: value-initialisation zeroes first when the default constructor is not user-provided.
struct Z { int i; virtual void f(); };
struct U { U(); int i; };
#ifdef B
U v = U();
#else
Z v = Z();
#endif
