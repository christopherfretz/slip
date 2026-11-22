#include "side.h"
// §10.3 StringEncoding `Ordinary`: the file name of a source location is an ordinary literal; its content differs here (no source reaches another encoding through a constant of a type, see Utf8).
namespace std { struct source_location { struct __impl { const char* _M_file_name; const char* _M_function_name; unsigned _M_line; unsigned _M_column; }; }; }
template <auto V> struct A;
#ifdef B
#line 7 "b.cpp"
A<__builtin_source_location()>* x;
#else
#line 7 "a.cpp"
A<__builtin_source_location()>* x;
#endif
