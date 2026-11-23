namespace n { void f(int); }
template <class T> void t(T) { n::f(1); }
void h() { t(1); t(2.0); }
