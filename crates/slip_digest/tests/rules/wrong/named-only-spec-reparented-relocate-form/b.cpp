template <class T> struct W1 { T a; };
template <class T> struct W2 { T *b; };
struct A {};
W1<A> w1;
void g(W2<A> *);
