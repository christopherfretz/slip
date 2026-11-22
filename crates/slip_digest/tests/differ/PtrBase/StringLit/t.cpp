#include "side.h"
// §8 PtrBase `StringLit`: the file name a source location points to, against the name of a variable of the same type.
namespace std { struct source_location { struct __impl { const char* _M_file_name; const char* _M_function_name; unsigned _M_line; unsigned _M_column; }; }; }
template <auto V> struct A;
#ifdef B
extern const char fname[];
constexpr std::source_location::__impl i{fname, fname, 3, 1};
A<&i>* x;
#else
A<__builtin_source_location()>* x;
#endif
