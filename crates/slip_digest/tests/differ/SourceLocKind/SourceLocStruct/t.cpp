#include "side.h"
// §7 SourceLocKind: `SourceLocStruct`: the fact differs.
namespace std { struct source_location { struct __impl { const char *_M_file_name; const char *_M_function_name; unsigned _M_line; unsigned _M_column; }; }; }
#ifdef B
const void *v = 0;
#else
const void *v = __builtin_source_location();
#endif
