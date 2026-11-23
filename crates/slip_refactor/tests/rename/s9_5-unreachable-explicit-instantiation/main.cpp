namespace n { template <class T> void f(T) {} }
template void n::f<int>(int);
void h() { n::f(1); }
