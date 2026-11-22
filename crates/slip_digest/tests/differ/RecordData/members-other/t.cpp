#include "side.h"
// §5.5 RecordData `members-other`: a member that is neither a field nor virtual.
#ifdef B
struct C { static void f(); };
#else
struct C { };
#endif
