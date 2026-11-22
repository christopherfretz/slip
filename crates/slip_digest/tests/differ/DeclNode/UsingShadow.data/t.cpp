#include "side.h"
// §5.3 DeclNode `UsingShadow.data` (keyed): the target is the discriminator.
namespace a { void x(int); void x(char); }
#ifdef B
namespace a { void x(long); } using a::x;
#else
using a::x;
#endif
