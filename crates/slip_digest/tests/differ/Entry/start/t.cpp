#include "side.h"
// §10.3 Entry `start`: the code units of a string literal (the file name a source location points to).
namespace std { struct source_location { struct __impl { const char* _M_file_name; const char* _M_function_name; unsigned _M_line; unsigned _M_column; }; }; }
template <auto V> struct A;
#ifdef B
#line 7 "abd.cpp"
A<__builtin_source_location()>* x;
#else
#line 7 "abc.cpp"
A<__builtin_source_location()>* x;
#endif
