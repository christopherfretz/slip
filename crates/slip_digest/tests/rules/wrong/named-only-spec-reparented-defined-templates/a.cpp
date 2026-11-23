template <class T> struct W1 { T a; };
template <class T> struct W2 { T *b; };
template <class> struct W3 {};
struct A {};
W3<A> w3;
void g(W1<A> *);
