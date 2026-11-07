import type { DeviceInfo } from "../types";

// Hardware identifiers verified against DeviceKit's device table:
// https://github.com/devicekit/DeviceKit/blob/master/Source/Device.generated.swift
const IPHONE_MODELS: Record<string, string> = {
  "iPhone3,1": "iPhone 4",
  "iPhone3,2": "iPhone 4",
  "iPhone3,3": "iPhone 4",
  "iPhone4,1": "iPhone 4s",
  "iPhone5,1": "iPhone 5",
  "iPhone5,2": "iPhone 5",
  "iPhone5,3": "iPhone 5c",
  "iPhone5,4": "iPhone 5c",
  "iPhone6,1": "iPhone 5s",
  "iPhone6,2": "iPhone 5s",
  "iPhone7,2": "iPhone 6",
  "iPhone7,1": "iPhone 6 Plus",
  "iPhone8,1": "iPhone 6s",
  "iPhone8,2": "iPhone 6s Plus",
  "iPhone9,1": "iPhone 7",
  "iPhone9,3": "iPhone 7",
  "iPhone9,2": "iPhone 7 Plus",
  "iPhone9,4": "iPhone 7 Plus",
  "iPhone8,4": "iPhone SE",
  "iPhone10,1": "iPhone 8",
  "iPhone10,4": "iPhone 8",
  "iPhone10,2": "iPhone 8 Plus",
  "iPhone10,5": "iPhone 8 Plus",
  "iPhone10,3": "iPhone X",
  "iPhone10,6": "iPhone X",
  "iPhone11,2": "iPhone XS",
  "iPhone11,4": "iPhone XS Max",
  "iPhone11,6": "iPhone XS Max",
  "iPhone11,8": "iPhone XR",
  "iPhone12,1": "iPhone 11",
  "iPhone12,3": "iPhone 11 Pro",
  "iPhone12,5": "iPhone 11 Pro Max",
  "iPhone12,8": "iPhone SE (2nd generation)",
  "iPhone13,2": "iPhone 12",
  "iPhone13,1": "iPhone 12 mini",
  "iPhone13,3": "iPhone 12 Pro",
  "iPhone13,4": "iPhone 12 Pro Max",
  "iPhone14,5": "iPhone 13",
  "iPhone14,4": "iPhone 13 mini",
  "iPhone14,2": "iPhone 13 Pro",
  "iPhone14,3": "iPhone 13 Pro Max",
  "iPhone14,6": "iPhone SE (3rd generation)",
  "iPhone14,7": "iPhone 14",
  "iPhone14,8": "iPhone 14 Plus",
  "iPhone15,2": "iPhone 14 Pro",
  "iPhone15,3": "iPhone 14 Pro Max",
  "iPhone15,4": "iPhone 15",
  "iPhone15,5": "iPhone 15 Plus",
  "iPhone16,1": "iPhone 15 Pro",
  "iPhone16,2": "iPhone 15 Pro Max",
  "iPhone17,3": "iPhone 16",
  "iPhone17,4": "iPhone 16 Plus",
  "iPhone17,1": "iPhone 16 Pro",
  "iPhone17,2": "iPhone 16 Pro Max",
  "iPhone17,5": "iPhone 16e",
  "iPhone18,3": "iPhone 17",
  "iPhone18,1": "iPhone 17 Pro",
  "iPhone18,2": "iPhone 17 Pro Max",
  "iPhone18,4": "iPhone Air",
  "iPhone18,5": "iPhone 17e",
  "iPhone19,2": "iPhone 18 Pro",
  "iPhone19,3": "iPhone 18 Pro Max",
  "iPhone19,7": "iPhone 18 Pro Max"
};

export function deviceModel(device: DeviceInfo, compact = false): string {
  const model = IPHONE_MODELS[device.product_type?.trim() ?? ""];
  const name = model ?? (device.device_name?.trim() || "iPhone");
  return compact ? name.replace("Pro Max", "PM") : name;
}

export function deviceDisplayName(device: DeviceInfo, peers: DeviceInfo[] = [], compact = false): string {
  const model = deviceModel(device, compact);
  const sameModel = peers.filter(peer => deviceModel(peer) === deviceModel(device));
  if (sameModel.length < 2) return model;
  const customName = device.device_name?.trim();
  if (customName && customName !== "iPhone" && customName !== deviceModel(device)
    && sameModel.filter(peer => peer.device_name?.trim() === customName).length === 1) {
    return `${model} · ${customName}`;
  }
  const number = [...sameModel].sort((a, b) => a.udid.localeCompare(b.udid)).findIndex(peer => peer.udid === device.udid) + 1;
  return `${model} · устройство ${number}`;
}

export function deviceSubtitle(device: DeviceInfo): string {
  const customName = device.device_name?.trim();
  return [customName && customName !== "iPhone" && customName !== deviceModel(device) ? customName : "",
    device.product_version ? `iOS ${device.product_version}` : ""].filter(Boolean).join(" · ");
}
