#include "side.h"
// §5.5 RecordData `defn`: declared against defined.
#ifdef B
struct C {};
#else
struct C;
#endif
