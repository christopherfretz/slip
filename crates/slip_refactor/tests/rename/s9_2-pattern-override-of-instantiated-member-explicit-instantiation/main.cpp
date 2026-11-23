namespace n {
template <class T> struct B { virtual void f() {} };
struct D : B<int> { void f() override; };
}
template void n::B<long>::f();
