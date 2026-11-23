#define CALL f(1) + 0
#define F(x) (f(x))
int f(int);
int y = CALL;
int z = F(2);
