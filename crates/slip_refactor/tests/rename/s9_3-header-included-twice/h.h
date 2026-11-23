namespace n { void f(int); }
typedef decltype(n::f(1)) R;
