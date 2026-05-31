Box type A = |
    value A,
|

get type A |this Box of A| -> A:
    return this.value
;
