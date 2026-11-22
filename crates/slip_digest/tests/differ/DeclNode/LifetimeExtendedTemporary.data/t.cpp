#include "side.h"
// §5.3 DeclNode `LifetimeExtendedTemporary.data`: the record is written (the temporaries' mangling indices).
struct P { const int &a; const int &b; const int &c; };
#ifdef B
P r{1, 2, 3};
#else
P r{1, 1, 1};
#endif
