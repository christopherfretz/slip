#include "side.h"
// §7 SourceLocValues: `location`: only the position differs.
namespace std { struct source_location { struct __impl { const char *_M_file_name; const char *_M_function_name; unsigned _M_line; unsigned _M_column; }; }; }
#ifdef B
#line 20
const void *v = __builtin_source_location();
#else
#line 10
const void *v = __builtin_source_location();
#endif
