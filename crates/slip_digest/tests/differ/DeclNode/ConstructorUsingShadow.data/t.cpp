#include "side.h"
// §5.3 DeclNode `ConstructorUsingShadow.data`: the target constructor.
#ifdef B
struct P { P(long); };
#else
struct P { P(int); };
#endif
struct D : P { using P::P; };
