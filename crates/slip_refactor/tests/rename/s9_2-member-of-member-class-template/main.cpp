namespace n { template <class T> struct S { template <class U> struct C { void f() {} }; }; }
void h() { n::S<int>::C<long> c; c.f(); }
