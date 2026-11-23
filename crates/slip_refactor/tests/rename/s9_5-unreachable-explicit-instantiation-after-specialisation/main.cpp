namespace n { template <class T> struct S { void f() {} };
template <> void S<int>::f() {}
template void S<int>::f(); }
