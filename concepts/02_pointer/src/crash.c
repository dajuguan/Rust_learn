// 有一个指向某个函数的指针，如果将其解引用成一个列表
#include "stdio.h"

void hello() {
    printf("Hello world!\n");
}

int main() {
    char buf[1024];
    void (* p)() = &hello;
    (*p)();
    int *p1 = (int *) p;
    p1[1] = 0xdeadbeef;
}