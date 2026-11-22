#include "side.h"
// §5.5 RecordData `members-fields`: two fields swapped (digest §5: field order is kept).
#ifdef B
struct C { int b; int a; };
#else
struct C { int a; int b; };
#endif
