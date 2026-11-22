#include "side.h"
// §2.3, §5.4 VarSpec `0-constinit`.
#ifdef B
extern int x; extern constinit int x;
#else
extern constinit int x; extern int x;
#endif
