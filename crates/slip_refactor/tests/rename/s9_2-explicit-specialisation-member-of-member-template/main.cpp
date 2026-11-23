namespace n { template <class T> struct S { template <class U> struct M { void f() {} }; };
template <> template <> void S<int>::M<long>::f() {} }
