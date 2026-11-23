namespace n { template <class T> struct Box { T t; }; void f(int); }
void h() { n::f(1); }
