namespace n { template <class T> struct S { void f() {} };
template void S<long>::f();
extern template void S<char>::f(); }
