import ctypes
import os
import time
from ctypes import *
from ctypes import wintypes

import cv2
import numpy as np
from win32gui import GetWindowRect, FindWindow

# 加载dll库
rs_dxgi = ctypes.CDLL("H:\\RustProjects\\dxgi4py_rs\\target\\release\\dxgi4py_rs.dll")
rs_dxgi.init_dxgi.argtypes = [wintypes.HWND]
rs_dxgi.init_dxgi.restype = ctypes.c_void_p

rs_dxgi.grab.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_uint, ctypes.c_uint, ctypes.c_uint,
                           ctypes.POINTER(ctypes.c_ubyte)]
rs_dxgi.grab.restype = ctypes.POINTER(ctypes.c_ubyte)

rs_dxgi.destroy.argtypes = [ctypes.c_void_p]
# rs_dxgi.free_buffer.argtypes = [ctypes.POINTER(ctypes.c_ubyte), ctypes.c_size_t]

dxgi = rs_dxgi
windll.user32.SetProcessDPIAware()

def test_dxgi(windowTitle):
    # 获取窗口hwnd
    # windowTitle = '尘白禁区'
    hwnd = FindWindow(None, windowTitle)

    # time.sleep(10)
    # 初始化
    handler = dxgi.init_dxgi(hwnd)
    print("handler: " + str(handler))
    # 指定截图区域(这里示例为截取整个窗口)
    left, top, right, bottom = DwmGet(hwnd)
    shotLeft, shotTop = 0, 0
    height = bottom - top
    width = right - left
    # 创建numpy array用于接收截图结果
    shot = np.ndarray((height, width, 4), dtype=np.uint8)
    shotPointer = shot.ctypes.data_as(POINTER(c_ubyte))
    expected_size = height * width * 4
    print(f"Python expected size: {expected_size}")
    startTime = time.time()
    # 截图
    print("test grab")
    os.makedirs('test_dxgi/' + windowTitle, exist_ok=True)
    i = 0
    # 截图速度与画面实际帧率一致，显存中未渲染完成的画面也会正常返回
    for i in range(0, 60):
        buffer = dxgi.grab(handler, shotLeft, shotTop, width, height, shotPointer)
        # 获取结果
        image = np.ctypeslib.as_array(buffer, shape=(height, width, 4))
        # cv2.imwrite(, image)
        cv2.imencode('.png', image)[1].tofile('test_dxgi/'+windowTitle+'/sample_pic' + str(i) + '.png')
    endTime = time.time()
    print('time cost: ' + str(endTime - startTime))
    # 转为BGR形式
    # img = cv2.cvtColor(image, cv2.COLOR_BGRA2BGR)
    # cv2.imshow('sample_pic', img)
    # cv2.waitKey(0)
    return handler

handler_qq = test_dxgi(windowTitle = '雷神加速器')
handler_phan = test_dxgi(windowTitle = 'MuMu安卓设备')

# 不再使用时销毁
# destroy操作在capture handler调用close时就会提前返回，这里最好等待一段时间让d3d device完全释放之后之后再结束程序，否则会报错
dxgi.destroy(handler_phan)
dxgi.destroy(handler_qq)

time.sleep(10)