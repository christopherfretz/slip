#include "side.h"
// §5.3 DeclNode `Using.data` (keyed): the qualifier is the discriminator.
namespace a { int x; } namespace b { int x; }
#ifdef B
using b::x;
#else
using a::x;
#endif
