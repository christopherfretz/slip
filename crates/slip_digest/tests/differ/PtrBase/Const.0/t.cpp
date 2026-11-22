#include "side.h"
// §8 PtrBase `Const.0`: the object pointed to, written in place: its line differs.
namespace std { struct source_location { struct __impl { const char* _M_file_name; const char* _M_function_name; unsigned _M_line; unsigned _M_column; }; }; }
template <auto V> struct A;
#ifdef B
#line 20
A<__builtin_source_location()>* x;
#else
#line 10
A<__builtin_source_location()>* x;
#endif
