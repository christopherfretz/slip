#include "side.h"
// §5.5 RecordData `members-virtual`: two virtual functions swapped (digest §5: virtual order is kept).
#ifdef B
struct C { virtual void g(); virtual void f(); };
#else
struct C { virtual void f(); virtual void g(); };
#endif
